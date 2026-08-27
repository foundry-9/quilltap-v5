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

- **Oracle baseline:** `f3892158d` — "feat(realtime): push interface
  updates over a WebSocket, tick clocks locally" (v4 main, 2026-08-26),
  adopted at the `f3892158d`-round unification (2026-08-26, the
  P4.D123→P4.D124 ∥ P4.D125 drift catch-up).
- **Checked:** 2026-08-26 (full `/driftcheck` — the fourteen post-baseline
  commits classified from their shipped hunks, superseding the
  unification's honest count-only entry).
- **v4 `main` HEAD at check:** `8872d7efc` — **FOURTEEN commits past the
  baseline**, all dated 2026-08-26: v4's 4.9.0 release push. Eight are
  **PORT** rows on already-ported surfaces (§3); six are **NO-PORT?**
  candidates, one of which (`dcab791c2`) is a 155-file
  claimed-behavior-neutral dedup sweep whose ratification is a
  P4.D31-class neutrality proof, not a reading.
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** since the last
  check, and re-measured this time: it is the bare
  `bugfix: started 4.8.4 bug branch` fork marker (2026-08-13) with **no
  fixes landed on it since**, so bugfix carries **no unabsorbed content**.
- **Checkout at check:** branch `main`, tree **clean**.
- **Verdict: DRIFT PENDING — 14 commits (8 PORT, 6 NO-PORT?).** A catch-up
  round is owed; three of the PORT rows are v4 bugs v5 measurably
  reproduces (bug 103 restore defaults, bug 104 the Z.AI vision list, the
  SQLite variable-limit ceiling — all confirmed present in v5 source at
  this check, see §3).
- **Regen rule in force: PIN REQUIRED.** v4 HEAD is fourteen commits past
  the baseline — every oracle regeneration must run from a lane-unique
  detached worktree pinned at `f3892158d` (recipe in §5.1;
  `recipe_sweep.py --v4 "$PIN"`), until a catch-up round absorbs the rows
  and moves the baseline.
- **💸 One banked live proof EXPIRES here (§5.5):** the standing dogfood
  item "the Z.AI refusal sentence" (banked at the `a14a1811` round) is
  killed by `964ffb959` — v4 **deletes** that refusal path, so the
  sentence can no longer be provoked once the row is ported. Retire the
  item rather than carrying it; the replacement proof is a `glm-5.3-*`
  attachment reaching the wire.

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
| `487ae57fe` | 2026-08-26 | test(coverage): regression tests for bugs 77, 83, 94, 99 and five uncovered modules | **NO-PORT?** | nine test files + docs; the ONLY lib/app hunks are a stated behaviour-neutral extraction on a ported surface (`useSSEStreaming.ts` → `useToolExecutionStatus.ts`, the bug-77 notice — v5 ported that notice as its own single-door surface in the `979652a9` round, so a v4-internal hook extraction has no v5 move). Ratify by verifying the extraction hunks are structure-only. One note to carry: the new `help-doc-chunks.repository` test pins the registerBlobColumns-re-assert trap — check v5's `help_doc_chunks` twin carries an equivalent pin | UNPROCESSED |
| `561466cfe` | 2026-08-26 | chore(dead-code): knip sweep — remove 11 unused exports, dedup hair guidance | **NO-PORT?** | pure deletions of unused v4 exports + a byte-identical HAIR-guidance dedup (v4 verified no prompt-text change, no IDENTITY_STACK_BUILDER_VERSION bump). No v5 behavior moves. Follow-up candidate, not a port: check whether v5 carries live twins of the deleted exports (`GROUP_WARDROBE_FOLDER`/`PROJECT_WARDROBE_FOLDER`, `resolveSharedWardrobeTiersForProject`, `noSharedWardrobeTiers` — P4.D71-era) that are now vestigial (`v5-refactor-vestigial-v4-cruft`) | UNPROCESSED |
| `7509c5cfb` | 2026-08-26 | Add missing @quilltap/plugin-types dependency to openai-compatible (#39) | **NO-PORT?** | dependency metadata only (`package.json` + a manifest version bump to plugin 1.0.42) + CHANGELOG; v4 states the bundle is unchanged. v5's hand-maintained `provider_manifest/manifests/*.json` carry no plugin version field, so nothing here reaches a generated manifest. Ratify by confirming the manifest hunk is the version line alone | UNPROCESSED |
| `21f573039` | 2026-08-26 | chore(logging): drop the per-publish realtime coalesce trace (#40) | **PORT** | one deleted line in `lib/realtime/bus.ts` — the `Realtime publish coalesced` debug print. v5 ported that bus **this round** (P4.D124): the trace is live at `crates/quilltap-core/src/realtime/bus.rs:202-203`. Delete the emit, KEEP the `coalesced` counter (the flush line reads it — v5's flush at `bus.rs:238` already does). A log-only change: no differential can see it (`differential-blind-to-a-log-only-fix`) — the pin is a capturing tracing layer asserting silence, the idiom P4.61 used | UNPROCESSED |
| `c0352fdba` | 2026-08-26 | docs(changelog): record the checklist item 9 package audit (#41) | **NO-PORT?** | `docs/CHANGELOG.md` only (+23/-0), an audit record; the package/manifest edits it originally carried dropped out in the rebase onto `7509c5cfb` | UNPROCESSED |
| `97d0b8f8e` | 2026-08-26 | refactor(themes): convert release-window Tailwind to qt-* utilities (checklist 7) (#42) | **PORT** | the P4.D117 (bugs 100/102) class again, on 15 v4 components: 20 sites to `qt-bg-*`, both `memory-recall-card` checkboxes to `qt-checkbox`, plus **two new utilities** — `.hover\:qt-bg-primary` and `.hover\:qt-bg-success`, the missing solid-fill siblings of the destructive one. v5's `apps/web/src/styles/qt-components/_utilities.css` defines only the fractional forms (`/5`, `/10`, `/15`), so the solid pair is absent and any v5 template using it is inert (the same "unwritten hover form" trap v4 names). TWO sites shift visibly and must be carried, not swept: `CharacterHeader.tsx:139` (avatar placeholder → `qt-bg-muted`) and `image-gallery.tsx:176` (overlay → `qt-bg-overlay-medium`, dropping a redundant `bg-opacity` pair). Also mirrors into `packages/theme-storybook` 1.0.65 (no v5 analog). Re-run `check-qt-classes` after | UNPROCESSED |
| `57e7b1bc2` | 2026-08-26 | Add comprehensive flag coverage tests for CLI completions (#43) | **PORT** | the `8f910137`/P4.D118 (bug 101) class: four documented flags tab-completion never offered — `docs docker-mounts --format` (all three shells, **and** bash's `vf_docs` value-flag list, or `--format json <TAB>` reads `json` as the verb — bug 101's exact failure mode) and `docs --uri`/`--base64` (fish). v5 byte-copies these templates at `crates/quilltap-cli/src/help/completion/{bash,zsh,fish}.template`; Tier R red-first, then `completion_behavior.rs`. v4's new `completion-coverage.test.js` also cross-checks bash `vf_*` against zsh `:value:` — worth mirroring as a v5 guard. `help/cli-completion.md` + `packages/quilltap/README.md` prose → the `p4.9i2` bank | UNPROCESSED |
| `e000d6bfc` | 2026-08-26 | fix(backup): seed the profile columns an older archive predates (bug 103) (#45) | **PORT** | **v5 measurably HAS this, on both columns.** v4's new `lib/llm/connection-profile-legacy-fields.ts` (`seedLegacyConnectionProfileFields`) is called from BOTH `restore.ts` and `import-profiles.ts` so the two paths land the same row: `supportsImageUpload` from the frozen historic provider map, `multiCharacterPrefill` as an explicit `null` ("never chosen"). v5's restore builds the row at `services/backup/restore/orchestrator.rs:275-300` — `supports_image_upload: b(p, "supportsImageUpload", false)` and `multi_character_prefill: ob(p, …)` (whose own comment correctly names the DDL-default mechanism it reproduces). v5's `.qtap` import already seeds half of it (`services/quilltap_import/profiles.rs:249` — `LEGACY_IMAGE_CAPABLE_PROVIDERS`), so the port is one shared helper + the restore call site, which also closes v5's own import/restore disagreement. Ported surfaces: the restore family (P4.9G/P4.D46) and the qtap import profiles (P4.D65-era). Bug 103 is v4's own release-checklist find, NOT a convergence | UNPROCESSED |
| `8440b6391` | 2026-08-26 | docs: 4.9.0 release notes and a documentation-freshness sweep (checklist item 13) (#44) | **NO-PORT?** + one tiny **PORT** | docs (release notes, API.md, DEVELOPMENT.md, README, v4's own CLAUDE.md) — all NO-PORT — **except** `app/about/AboutView.tsx`, whose provider list gains DeepSeek, Z.AI and NanoGPT. v5's `apps/web/src/app/screens/about/about-page.ts:398` still reads the old seven-provider sentence, so that string needs the same edit (the About vertical, `p4.9`-era). API.md's three corrected paths are documentation of routes v5 already implements — read them as a cross-check, not a port | UNPROCESSED |
| `dcab791c2` | 2026-08-26 | refactor: dedup sweep over everything touched since 4.8.0 (checklist item 3) | **NO-PORT?** (ratify by NEUTRALITY PROOF, not by reading) | the P4.D31/P4.D44 release-refactor class, and the round's largest risk: 155 files, ~2,800 lines removed, across route factories, hooks, tool handlers, the data layer, components, **and the provider wire** (`@quilltap/plugin-utils` 2.5.0 shares one `buildRequestBody` between send/stream; nanogpt 1.1.1 single-sources base URL + image MIME list; ollama 1.0.46 dedups the think-retry and request build). v4 claims "same strings, status codes, wire bodies, and log lines" and even pins the byte order explicitly (the `stream`/`stream_options` keys are SPREAD mid-literal to keep their historical position between `stop` and `user`) — which is the tell that this is exactly where a claimed-neutral sweep can move a byte. **Ratification = regenerate the affected families at the new pin and prove them byte-identical** (`request-envelopes`, the ollama/nanogpt/OAC wire corpora, the wardrobe + scenario route families, the title/`cleanTitle` families). Two hunks are NOT neutral and need their own answer: the scenarios POST "latent crash — `repos` referenced without destructuring" (a JS-only defect; confirm v5 has no analog) and the collapsed twins (`resolveDefaultOutfit` → `buildDefaultOutfit`, the help/normal chat-title pair) — collapsing twins is only neutral if the twins agreed, which is a measurement | UNPROCESSED |
| `914b59e13` | 2026-08-26 | refactor(backup): route full-wipe memory deletion through the memory-gate chokepoint | **PORT** | the replace-mode restore / delete-all path deleted memories with per-row `repos.memories.delete` calls — the last bypass of `deleteMemoriesWithUnlinkBatch`. It now collects all doomed ids and makes ONE batch call (in the full-wipe case the neighbour scrub is a no-op, every neighbour being doomed too), with a regression test asserting the direct repository delete is never hit. v5's twins: `db/memories.rs` (`bulk_delete` at :332, the unlink batch at ~:560-630) and the restore/delete-all service ported in P4.30/P4.31. Stacks with `805ef12bf` — port them together, chokepoint first | UNPROCESSED |
| `805ef12bf` | 2026-08-26 | fix(memory): chunk batch memory deletion under the SQLite variable limit | **PORT** | **v5 measurably HAS this, at both sites.** v4 chunks ids 900 at a time through a new `lib/utils/chunk.ts` in `deleteMemoriesWithUnlinkBatch`'s id→character resolve and in the repository's `bulkDelete`. v5 builds one `IN (…)` with one bind per id at `db/memories.rs:336-345` (`bulk_delete`) and again at `:607-617` (the doomed-set resolve) — the same `SQLITE_MAX_VARIABLE_NUMBER` ceiling, and SQLite3MC is the same engine. Bites full-wipe restores and large character cascades on instances with tens of thousands of memories (Friday's scale). The pin is v4's shape: 2,000-id batches asserting the chunked query shapes | UNPROCESSED |
| `964ffb959` | 2026-08-26 | fix(z-ai): drop the plugin's private vision list so GLM 5.3 receives images (bug 104) | **PORT** | **v5 measurably HAS this.** Bug 91's shape, third instance: the Z.AI plugin's own `VISION_MODEL_PATTERNS` (`/^glm-\d+(\.\d+)?v/i`) failed `glm-5.3-flash`, so attachments were dropped before the wire with "Selected Z.AI model does not support image input" **while `supportsImageUpload` had already suppressed the describe-fallback** — the character never saw the Lantern background, and bug 94's warning toast fired every following turn. v4 DELETES `VISION_MODEL_PATTERNS`/`isVisionModel` and the vision branch of `buildUserContent` (MIME check + missing-data check remain), dropping `formatMessages`' now-unused `model` param. v5's transcription is `model/request_builder/chat_completions.rs:54-57` (`is_zai_vision_model`), the sentence at `:106`, the branch at `:864` — all three go. Restores bug 91's rule (host answers *does the model read images*, plugin answers *can the transport send them*) and matches the shape NanoGPT took at P4.D106. v4's own find (live Friday, 2026-08-26), NOT a convergence. **See §1: this row expires a banked 💸 proof** | UNPROCESSED |
| `8872d7efc` | 2026-08-26 | perf(cheap-llm): give compression its own budget, and log cheap-task failures | **PORT** | the P4.D42 bounded-request surface moves: the three compression task types get **75 s**, every other cheap task keeps 45 s, and local providers keep 180 s **checked first**, so a per-task override can never shrink the local budget. `providerBudgetFor` now threads the task type too (staying 5 s inside whichever deadline applies), and `runCheapLLMTask`'s duplicate local `deadlineFor` closure is deleted. Plus a new warn on a failed cheap task naming task type / provider / model / chat / character — previously a provider giving up on its own budget arrived as an ordinary provider error, invisible to a server-log grep. v5's twins are flat constants: `services/cheap_llm_exec.rs:143` (`CHEAP_LLM_TASK_TIMEOUT_MS = 45_000`), `:149` (local 180 s), `cheap_llm_deadline_for` at `:160` and `provider_budget_for` at `:176` — both take only the selection, so the signature grows a task-type argument. Watch the timeout MESSAGE bytes (`cheap_llm_timeout_message`, pinned at `:1056-1065`): a compression timeout now reads 75000ms | UNPROCESSED |

**Row notes** (delineation detail for `/setupphase`; classified from the
shipped hunks, not the messages):

- **No convergence rows in this block.** `docs/developer/bugs.md` moved twice
  (bugs 103 and 104), but both are v4's OWN finds — 103 from release checklist
  item 10, 104 from a live Friday chat on 2026-08-26 — not v4 adopting a fix
  this port filed. No both-directions pin should trip at the baseline move on
  their account.
- **Three rows are bugs v5 REPRODUCES**, confirmed by reading v5 source at this
  check, not inferred: `e000d6bfc` (bug 103, `restore/orchestrator.rs:291`),
  `964ffb959` (bug 104, `chat_completions.rs:57`), `805ef12bf` (the SQLite
  variable ceiling, `db/memories.rs:336` and `:607`). Each is red-first
  portable: measure v5 failing before fixing it.
- **The 4.9.0 release shape.** All fourteen rows are one day's release push
  (v4 `package.json` is `4.9.0-dev.*` throughout; `docs/releases/4.9.0.md` is
  drafted but flagged for the human's review). Expect a `release: 4.9.0`
  squash and a `bugfix: started 4.9.0 bug branch` fork soon after — re-probe
  BOTH branches at the next check rather than assuming bugfix is still inert.
- **Two rows stack:** `914b59e13` (route full-wipe deletion through the
  memory-gate batch chokepoint) and `805ef12bf` (chunk that batch under the
  variable limit) are the same code path, landed in that order. Port the
  chokepoint first, or the chunking lands on a call site that is about to move.
- **Suggested lane shape** (for `/setupphase`, not a commitment): a memory /
  backup lane (`914b59e13` + `805ef12bf` + `e000d6bfc`); a provider lane
  (`964ffb959` + `8872d7efc` + the `21f573039` trace); a client/CLI lane
  (`97d0b8f8e` + `57e7b1bc2` + the AboutView string from `8440b6391`); and
  `dcab791c2`'s neutrality proof as its own lane, since it is a sweep over
  everything the other three touch.

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
